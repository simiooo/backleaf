// The decoder only receives streams. All output paths and file handles belong to this host.
#define NOMINMAX
#include <windows.h>
#include <winternl.h>
#include <propvarutil.h>
#include <atomic>
#include <algorithm>
#include <array>
#include <cstdint>
#include <cstdio>
#include <cwctype>
#include <iostream>
#include <map>
#include <memory>
#include <mutex>
#include <set>
#include <stdexcept>
#include <string>
#include <vector>
#include "CPP/Common/MyInitGuid.h"
#include "CPP/7zip/Archive/IArchive.h"
#include "CPP/7zip/IPassword.h"

constexpr uint64_t MaxBytes=1ULL<<40;
constexpr uint32_t MaxItems=1000000;
std::mutex reportMutex;
std::atomic<uint64_t> written{0},finished{0};
uint64_t totalItems=0,totalBytes=0,lastNotice=0;
std::wstring failure;
struct Handle {
  HANDLE h=INVALID_HANDLE_VALUE;
  Handle()=default; explicit Handle(HANDLE value):h(value){}
  Handle(const Handle&)=delete; Handle& operator=(const Handle&)=delete;
  Handle(Handle&& v) noexcept:h(v.h){v.h=INVALID_HANDLE_VALUE;}
  ~Handle(){if(h!=INVALID_HANDLE_VALUE&&h)CloseHandle(h);}
};
void require(bool ok,const wchar_t* message){if(!ok){failure=message;throw std::runtime_error("archive");}}
std::string utf8(const std::wstring& s){int n=WideCharToMultiByte(CP_UTF8,WC_ERR_INVALID_CHARS,s.data(),(int)s.size(),nullptr,0,nullptr,nullptr);require(n>0||s.empty(),L"文件名编码无效");std::string r(n,0);if(n)WideCharToMultiByte(CP_UTF8,WC_ERR_INVALID_CHARS,s.data(),(int)s.size(),r.data(),n,nullptr,nullptr);return r;}
std::string json(const std::wstring& text,bool full=false){std::string r="\"";for(unsigned char c:utf8([&]{auto s=full?text:text.substr(0,256);if(!full&&!s.empty()&&s.back()>=0xd800&&s.back()<=0xdbff)s.pop_back();return s;}())){if(c=='"'||c=='\\'){r+='\\';r+=(char)c;}else if(c<32){char b[7];sprintf_s(b,"\\u%04x",c);r+=b;}else r+=(char)c;}return r+'"';}
void progress(const std::wstring& name,bool force=false){std::lock_guard<std::mutex>lock(reportMutex);auto now=GetTickCount64();if(!force&&now-lastNotice<100)return;lastNotice=now;std::cout<<"{\"type\":\"progress\",\"done\":"<<finished.load()<<",\"bytes\":"<<written.load()<<",\"name\":"<<json(name)<<"}\n"<<std::flush;}
void plan(){std::cout<<"{\"type\":\"plan\",\"count\":"<<totalItems<<",\"bytes\":"<<totalBytes<<"}\n"<<std::flush;}
std::array<unsigned char,33> identity(HANDLE h){BY_HANDLE_FILE_INFORMATION basic{};require(GetFileInformationByHandle(h,&basic),L"无法核实文件身份");FILE_ID_INFO id{};std::array<unsigned char,33>r{};if(GetFileInformationByHandleEx(h,FileIdInfo,&id,sizeof(id))){r[0]=1;memcpy(r.data()+1,&id.VolumeSerialNumber,8);memcpy(r.data()+9,id.FileId.Identifier,16);}else{uint64_t volume=basic.dwVolumeSerialNumber,file=((uint64_t)basic.nFileIndexHigh<<32)|basic.nFileIndexLow;memcpy(r.data()+1,&volume,8);memcpy(r.data()+9,&file,8);}uint64_t created=((uint64_t)basic.ftCreationTime.dwHighDateTime<<32)|basic.ftCreationTime.dwLowDateTime;memcpy(r.data()+25,&created,8);return r;}
std::wstring hex(const std::array<unsigned char,33>& a){const wchar_t*c=L"0123456789abcdef";std::wstring r;for(auto v:a){r+=c[v>>4];r+=c[v&15];}return r;}
Handle openRead(const std::wstring& path){Handle h(CreateFileW(path.c_str(),GENERIC_READ,FILE_SHARE_READ,nullptr,OPEN_EXISTING,FILE_FLAG_OPEN_REPARSE_POINT,nullptr));require(h.h!=INVALID_HANDLE_VALUE,L"源文件不可读取或正被修改");BY_HANDLE_FILE_INFORMATION b{};require(GetFileInformationByHandle(h.h,&b)&&!(b.dwFileAttributes&FILE_ATTRIBUTE_REPARSE_POINT),L"不支持链接源文件");return h;}
void readExact(HANDLE h,void* data,DWORD n){DWORD read=0;require(ReadFile(h,data,n,&read,nullptr)&&read==n,L"归档清单损坏");}
uint64_t seek(HANDLE h,int64_t n,DWORD origin){LARGE_INTEGER d{},r{};d.QuadPart=n;require(SetFilePointerEx(h,d,&r,origin),L"无法定位文件内容");return r.QuadPart;}
std::wstring lower(std::wstring s){for(auto&c:s)c=(wchar_t)towlower(c);return s;}
std::vector<std::wstring> safe(const std::wstring& path){require(!path.empty()&&path.size()<32700&&path[0]!=L'/'&&path[0]!=L'\\',L"压缩包含不安全路径");utf8(path);std::vector<std::wstring>parts;size_t from=0;for(size_t i=0;i<=path.size();i++){if(i!=path.size()&&path[i]!=L'/'&&path[i]!=L'\\')continue;auto p=path.substr(from,i-from);from=i+1;if(p.empty()&&i==path.size())break;require(!p.empty()&&p!=L"."&&p!=L".."&&p.size()<=255&&p.back()!=L'.'&&p.back()!=L' ',L"压缩包含不安全路径");for(wchar_t c:p)require(c>=32&&std::wstring(L":<>|?*\"").find(c)==std::wstring::npos,L"文件名不适用于 Windows");auto stem=lower(p.substr(0,p.find(L'.')));require(stem!=L"con"&&stem!=L"prn"&&stem!=L"aux"&&stem!=L"nul"&&stem!=L"conin$"&&stem!=L"conout$"&&!(stem.size()==4&&(stem.rfind(L"com",0)==0||stem.rfind(L"lpt",0)==0)&&((stem[3]>=L'1'&&stem[3]<=L'9')||stem[3]==L'¹'||stem[3]==L'²'||stem[3]==L'³')),L"压缩包含设备名称");parts.push_back(p);require(parts.size()<=1024,L"目录层级过深");}require(!parts.empty(),L"压缩包路径为空");return parts;}
std::wstring normalized(const std::wstring& p){std::wstring r;for(auto&c:safe(p)){if(!r.empty())r+=L'\\';r+=lower(c);}return r;}

#define REFS std::atomic<ULONG>refs{1}; ULONG STDMETHODCALLTYPE AddRef()noexcept override{return ++refs;} ULONG STDMETHODCALLTYPE Release()noexcept override{auto n=--refs;if(!n)delete this;return n;}
class InStream final:public IInStream {
public: REFS
  Handle file;uint64_t position=0,maximum=0;bool track;std::wstring name;
  InStream(Handle h,bool t=false,std::wstring n=L""):file(std::move(h)),track(t),name(std::move(n)){}
  HRESULT STDMETHODCALLTYPE QueryInterface(REFIID id,void**out)noexcept override{*out=nullptr;if(id==IID_IUnknown||id==IID_IInStream||id==IID_ISequentialInStream){*out=static_cast<IInStream*>(this);AddRef();return S_OK;}return E_NOINTERFACE;}
  HRESULT STDMETHODCALLTYPE Read(void*data,UInt32 size,UInt32*processed)noexcept override{DWORD n=0;if(!ReadFile(file.h,data,size,&n,nullptr))return HRESULT_FROM_WIN32(GetLastError());if(processed)*processed=n;position+=n;if(track&&position>maximum){written.fetch_add(position-maximum);maximum=position;progress(name);}return S_OK;}
  HRESULT STDMETHODCALLTYPE Seek(Int64 offset,UInt32 origin,UInt64*newPos)noexcept override{try{position=seek(file.h,offset,origin);if(newPos)*newPos=position;return S_OK;}catch(...){return E_FAIL;}}
};
class OutStream final:public IOutStream {
public: REFS
  Handle file;bool track;uint64_t expected=0,used=0;std::wstring name;std::vector<Handle>parents;
  OutStream(Handle h,bool t,uint64_t e,std::wstring n,std::vector<Handle>p={}):file(std::move(h)),track(t),expected(e),name(std::move(n)),parents(std::move(p)){}
  HRESULT STDMETHODCALLTYPE QueryInterface(REFIID id,void**out)noexcept override{*out=nullptr;if(id==IID_IUnknown||id==IID_IOutStream||id==IID_ISequentialOutStream){*out=static_cast<IOutStream*>(this);AddRef();return S_OK;}return E_NOINTERFACE;}
  HRESULT STDMETHODCALLTYPE Write(const void*data,UInt32 size,UInt32*processed)noexcept override{if(track&&(size>expected-used||written.load()>MaxBytes-size))return E_ABORT;DWORD n=0;if(!WriteFile(file.h,data,size,&n,nullptr))return HRESULT_FROM_WIN32(GetLastError());if(processed)*processed=n;used+=n;if(track){written.fetch_add(n);progress(name);}return S_OK;}
  HRESULT STDMETHODCALLTYPE Seek(Int64 offset,UInt32 origin,UInt64*newPos)noexcept override{try{auto p=seek(file.h,offset,origin);if(newPos)*newPos=p;return S_OK;}catch(...){return E_FAIL;}}
  HRESULT STDMETHODCALLTYPE SetSize(UInt64 n)noexcept override{if(n>MaxBytes)return E_ABORT;try{auto p=seek(file.h,0,FILE_CURRENT);seek(file.h,n,FILE_BEGIN);if(!SetEndOfFile(file.h))return E_FAIL;seek(file.h,p,FILE_BEGIN);return S_OK;}catch(...){return E_FAIL;}}
};
struct Property {PROPVARIANT v{};~Property(){PropVariantClear(&v);}};
std::wstring itemPath(IInArchive*arc,UInt32 index){Property p;require(SUCCEEDED(arc->GetProperty(index,kpidPath,&p.v))&&p.v.vt==VT_BSTR,L"压缩包缺少文件名");return std::wstring(p.v.bstrVal,SysStringLen(p.v.bstrVal));}
bool boolProp(IInArchive*arc,UInt32 index,PROPID id){Property p;require(SUCCEEDED(arc->GetProperty(index,id,&p.v)),L"无法读取压缩包属性");return p.v.vt==VT_BOOL&&p.v.boolVal!=VARIANT_FALSE;}
uint64_t sizeProp(IInArchive*arc,UInt32 index){Property p;require(SUCCEEDED(arc->GetProperty(index,kpidSize,&p.v)),L"无法读取展开大小");if(p.v.vt==VT_EMPTY&&boolProp(arc,index,kpidIsDir))return 0;require(p.v.vt==VT_UI8,L"展开大小未知");return p.v.uhVal.QuadPart;}
void validate(IInArchive*arc,UInt32 index){auto path=itemPath(arc,index);safe(path);require(!boolProp(arc,index,kpidEncrypted),L"压缩包需要密码");for(auto id:{kpidSymLink,kpidHardLink,kpidNtReparse,kpidIsAltStream,kpidIsAnti}){Property p;require(SUCCEEDED(arc->GetProperty(index,id,&p.v)),L"无法读取归档属性");require(p.v.vt==VT_EMPTY||(p.v.vt==VT_BSTR&&SysStringLen(p.v.bstrVal)==0)||(p.v.vt==VT_BOOL&&!p.v.boolVal)||(p.v.vt==VT_UI4&&!p.v.ulVal),L"不提取链接、流或反向条目");}for(auto id:{kpidAttrib,kpidPosixAttrib}){Property p;arc->GetProperty(index,id,&p.v);if(p.v.vt==VT_UI4){if(id==kpidAttrib)require(!(p.v.ulVal&FILE_ATTRIBUTE_REPARSE_POINT),L"不提取重解析点");else{auto kind=p.v.ulVal&0170000;require(kind==0||kind==0100000||kind==0040000,L"不提取特殊文件");}}}}

using NtCreate=NTSTATUS(NTAPI*)(PHANDLE,ACCESS_MASK,POBJECT_ATTRIBUTES,PIO_STATUS_BLOCK,PLARGE_INTEGER,ULONG,ULONG,ULONG,ULONG,PVOID,ULONG);
class OutputRoot {
  Handle root;std::map<std::wstring,std::array<unsigned char,33>>known;NtCreate create;
public:
  OutputRoot(const std::wstring&path,const std::wstring&expected):root(CreateFileW(path.c_str(),FILE_LIST_DIRECTORY|FILE_READ_ATTRIBUTES|SYNCHRONIZE,FILE_SHARE_READ,nullptr,OPEN_EXISTING,FILE_FLAG_BACKUP_SEMANTICS|FILE_FLAG_OPEN_REPARSE_POINT,nullptr)) {
    require(root.h!=INVALID_HANDLE_VALUE&&hex(identity(root.h))==expected,L"暂存目录已改变");BY_HANDLE_FILE_INFORMATION b{};require(GetFileInformationByHandle(root.h,&b)&&!(b.dwFileAttributes&FILE_ATTRIBUTE_REPARSE_POINT),L"暂存目录不可用");create=(NtCreate)GetProcAddress(GetModuleHandleW(L"ntdll.dll"),"NtCreateFile");require(create!=nullptr,L"系统不支持安全提取");
  }
  Handle child(HANDLE parent,const std::wstring&name,bool directory,bool exists){UNICODE_STRING text{};text.Buffer=(PWSTR)name.data();text.Length=(USHORT)(name.size()*2);text.MaximumLength=text.Length;OBJECT_ATTRIBUTES attrs{};InitializeObjectAttributes(&attrs,&text,OBJ_CASE_INSENSITIVE|0x1000,parent,nullptr);IO_STATUS_BLOCK io{};HANDLE h=nullptr;auto status=create(&h,(directory?FILE_LIST_DIRECTORY:FILE_WRITE_DATA|FILE_WRITE_ATTRIBUTES)|FILE_READ_ATTRIBUTES|SYNCHRONIZE,&attrs,&io,nullptr,FILE_ATTRIBUTE_NORMAL,FILE_SHARE_READ,exists?1:2,(directory?1:0x40)|0x20|0x200000,nullptr,0);require(status>=0,L"目标路径冲突、无权限或包含链接");Handle result(h);BY_HANDLE_FILE_INFORMATION b{};require(GetFileInformationByHandle(h,&b)&&!(b.dwFileAttributes&FILE_ATTRIBUTE_REPARSE_POINT),L"目标路径包含链接");return result;}
  ISequentialOutStream* stream(const std::wstring&name,bool directory,uint64_t size){auto parts=safe(name);HANDLE parent=root.h;std::vector<Handle>held;std::wstring prefix;for(size_t i=0;i<parts.size();i++){if(!prefix.empty())prefix+=L'\\';prefix+=lower(parts[i]);bool dir=i+1<parts.size()||directory;bool exists=dir&&known.count(prefix);auto h=child(parent,parts[i],dir,exists);if(dir){auto id=identity(h.h);if(exists)require(known[prefix]==id,L"目标目录已被替换");else known.emplace(prefix,id);parent=h.h;held.push_back(std::move(h));}else{return new OutStream(std::move(h),true,size,name,std::move(held));}}return nullptr;}
};
class OpenCallback final:public IArchiveOpenCallback,public ICryptoGetTextPassword {
public:REFS
  HRESULT STDMETHODCALLTYPE QueryInterface(REFIID id,void**p)noexcept override{*p=nullptr;if(id==IID_IUnknown||id==IID_IArchiveOpenCallback)*p=static_cast<IArchiveOpenCallback*>(this);else if(id==IID_ICryptoGetTextPassword)*p=static_cast<ICryptoGetTextPassword*>(this);if(*p){AddRef();return S_OK;}return E_NOINTERFACE;}
  HRESULT STDMETHODCALLTYPE SetTotal(const UInt64*,const UInt64*)noexcept override{return S_OK;}
  HRESULT STDMETHODCALLTYPE SetCompleted(const UInt64*,const UInt64*)noexcept override{return S_OK;}
  HRESULT STDMETHODCALLTYPE CryptoGetTextPassword(BSTR*)noexcept override{failure=L"压缩包需要密码";return E_ABORT;}
};
class ExtractCallback final:public IArchiveExtractCallback,public ICryptoGetTextPassword {
public:REFS
  IInArchive*arc;OutputRoot&root;std::wstring current;UInt32 currentIndex=0;OutStream*held=nullptr;
  ~ExtractCallback(){if(held)held->Release();}
  ExtractCallback(IInArchive*a,OutputRoot&r):arc(a),root(r){}
  HRESULT STDMETHODCALLTYPE QueryInterface(REFIID id,void**p)noexcept override{*p=nullptr;if(id==IID_IUnknown||id==IID_IArchiveExtractCallback||id==IID_IProgress)*p=static_cast<IArchiveExtractCallback*>(this);else if(id==IID_ICryptoGetTextPassword)*p=static_cast<ICryptoGetTextPassword*>(this);if(*p){AddRef();return S_OK;}return E_NOINTERFACE;}
  HRESULT STDMETHODCALLTYPE SetTotal(UInt64)noexcept override{return S_OK;}
  HRESULT STDMETHODCALLTYPE SetCompleted(const UInt64*)noexcept override{return S_OK;}
  HRESULT STDMETHODCALLTYPE CryptoGetTextPassword(BSTR*)noexcept override{failure=L"压缩包需要密码";return E_ABORT;}
  HRESULT STDMETHODCALLTYPE GetStream(UInt32 i,ISequentialOutStream**out,Int32 mode)noexcept override{*out=nullptr;try{if(mode!=0)return S_OK;validate(arc,i);current=itemPath(arc,i);currentIndex=i;*out=root.stream(current,boolProp(arc,i,kpidIsDir),sizeProp(arc,i));held=static_cast<OutStream*>(*out);if(held)held->AddRef();progress(current,true);return S_OK;}catch(...){return E_FAIL;}}
  HRESULT STDMETHODCALLTYPE PrepareOperation(Int32)noexcept override{return S_OK;}
  HRESULT STDMETHODCALLTYPE SetOperationResult(Int32 result)noexcept override{if(result!=0){failure=L"压缩包损坏、校验失败或格式不支持";return E_FAIL;}if(held){Property time;arc->GetProperty(currentIndex,kpidMTime,&time.v);if(time.v.vt==VT_FILETIME&&!SetFileTime(held->file.h,nullptr,nullptr,&time.v.filetime)){failure=L"无法保存文件时间";return E_FAIL;}if(!FlushFileBuffers(held->file.h)){failure=L"无法写入目标文件";return E_FAIL;}held->Release();held=nullptr;}finished.fetch_add(1);progress(current);return S_OK;}
};
struct Record {std::array<unsigned char,33>id;uint64_t size,modified;bool directory;std::wstring source,name;};
class Manifest {
  Handle file;std::vector<uint64_t>offsets;std::mutex mutex;
  std::wstring string(){uint32_t n=0;readExact(file.h,&n,4);require(n&&n<32760,L"归档路径过长");std::wstring r(n,0);readExact(file.h,r.data(),n*2);require(r.find(L'\0')==std::wstring::npos,L"归档路径无效");return r;}
  Record read(){Record r{};readExact(file.h,r.id.data(),33);readExact(file.h,&r.size,8);readExact(file.h,&r.modified,8);uint8_t d;readExact(file.h,&d,1);require(d<2,L"归档清单无效");r.directory=d;r.source=string();r.name=string();return r;}
public:
  Manifest(const std::wstring&path,const std::wstring&expected):file(openRead(path)){
    require(hex(identity(file.h))==expected,L"归档清单已改变");char magic[8];readExact(file.h,magic,8);require(memcmp(magic,"BLARCH01",8)==0,L"归档清单无效");readExact(file.h,&totalItems,8);require(totalItems<=MaxItems,L"项目超过一百万上限");std::set<std::wstring>names;uint64_t units=0;
    for(uint64_t i=0;i<totalItems;i++){offsets.push_back(seek(file.h,0,FILE_CURRENT));auto r=read();units+=r.name.size()+r.source.size();require(units<64ULL*1024*1024,L"路径清单超过资源上限");require(names.insert(normalized(r.name)).second,L"文件名在 Windows 上冲突");require(r.size<=MaxBytes-totalBytes,L"归档超过 1 TiB 上限");if(!r.directory)totalBytes+=r.size;}
  }
  Record get(UInt32 index){std::lock_guard<std::mutex>lock(mutex);require(index<offsets.size(),L"归档项目无效");seek(file.h,offsets[index],FILE_BEGIN);return read();}
};
class UpdateCallback final:public IArchiveUpdateCallback {
public:REFS
  Manifest&manifest;std::wstring current;
  explicit UpdateCallback(Manifest&m):manifest(m){}
  HRESULT STDMETHODCALLTYPE QueryInterface(REFIID id,void**p)noexcept override{*p=nullptr;if(id==IID_IUnknown||id==IID_IArchiveUpdateCallback||id==IID_IProgress){*p=static_cast<IArchiveUpdateCallback*>(this);AddRef();return S_OK;}return E_NOINTERFACE;}
  HRESULT STDMETHODCALLTYPE SetTotal(UInt64)noexcept override{return S_OK;}
  HRESULT STDMETHODCALLTYPE SetCompleted(const UInt64*)noexcept override{return S_OK;}
  HRESULT STDMETHODCALLTYPE GetUpdateItemInfo(UInt32,Int32*data,Int32*props,UInt32*old)noexcept override{*data=1;*props=1;*old=(UInt32)-1;return S_OK;}
  HRESULT STDMETHODCALLTYPE GetProperty(UInt32 i,PROPID id,PROPVARIANT*v)noexcept override{try{PropVariantInit(v);auto r=manifest.get(i);switch(id){case kpidPath:v->vt=VT_BSTR;v->bstrVal=SysAllocStringLen(r.name.data(),(UINT)r.name.size());return v->bstrVal?S_OK:E_OUTOFMEMORY;case kpidIsDir:v->vt=VT_BOOL;v->boolVal=r.directory?VARIANT_TRUE:VARIANT_FALSE;break;case kpidSize:v->vt=VT_UI8;v->uhVal.QuadPart=r.size;break;case kpidAttrib:v->vt=VT_UI4;v->ulVal=r.directory?FILE_ATTRIBUTE_DIRECTORY:FILE_ATTRIBUTE_ARCHIVE;break;case kpidMTime:v->vt=VT_FILETIME;v->filetime.dwLowDateTime=(DWORD)r.modified;v->filetime.dwHighDateTime=(DWORD)(r.modified>>32);break;default:break;}return S_OK;}catch(...){return E_FAIL;}}
  HRESULT STDMETHODCALLTYPE GetStream(UInt32 i,ISequentialInStream**stream)noexcept override{*stream=nullptr;try{auto r=manifest.get(i);current=r.name;if(r.directory)return S_OK;auto h=openRead(r.source);require(identity(h.h)==r.id,L"源文件已改变");BY_HANDLE_FILE_INFORMATION b{};require(GetFileInformationByHandle(h.h,&b),L"源文件不可读取");require((((uint64_t)b.nFileSizeHigh<<32)|b.nFileSizeLow)==r.size&&(((uint64_t)b.ftLastWriteTime.dwHighDateTime<<32)|b.ftLastWriteTime.dwLowDateTime)==r.modified,L"源文件内容已改变");*stream=new InStream(std::move(h),true,r.name);progress(current,true);return S_OK;}catch(...){return E_FAIL;}}
  HRESULT STDMETHODCALLTYPE SetOperationResult(Int32 result)noexcept override{if(result){failure=L"压缩失败";return E_FAIL;}finished.fetch_add(1);progress(current);return S_OK;}
};
using Factory=HRESULT(WINAPI*)(const GUID*,const GUID*,void**);
GUID format(unsigned char id){return GUID{0x23170F69,0x40C1,0x278A,{0x10,0,0,1,0x10,id,0,0}};}
int wmain(int argc,wchar_t**argv){
  try {
    require(argc==9,L"归档参数无效");SetErrorMode(SEM_FAILCRITICALERRORS|SEM_NOGPFAULTERRORBOX);
    auto library=LoadLibraryExW(argv[4],nullptr,LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR|LOAD_LIBRARY_SEARCH_SYSTEM32);require(library!=nullptr,L"归档组件不可用");auto factory=(Factory)GetProcAddress(library,"CreateObject");require(factory!=nullptr,L"归档组件不兼容");
    if(std::wstring(argv[1])==L"create"){
      Manifest manifest(argv[2],argv[5]);plan();Handle output(CreateFileW(argv[3],GENERIC_WRITE|GENERIC_READ,FILE_SHARE_READ,nullptr,OPEN_EXISTING,FILE_FLAG_OPEN_REPARSE_POINT,nullptr));require(output.h!=INVALID_HANDLE_VALUE&&hex(identity(output.h))==argv[6],L"暂存文件已改变");LARGE_INTEGER length{};require(GetFileSizeEx(output.h,&length)&&length.QuadPart==0,L"暂存文件不为空");IOutArchive*arc=nullptr;auto id=format(1);require(SUCCEEDED(factory(&id,&IID_IOutArchive,(void**)&arc)),L"ZIP 引擎不可用");auto stream=new OutStream(std::move(output),false,0,L"");auto callback=new UpdateCallback(manifest);auto result=arc->UpdateItems(stream,(UInt32)totalItems,callback);stream->Release();callback->Release();arc->Release();require(result==S_OK,L"压缩未完成");
    }else{
      auto source=openRead(argv[2]);require(hex(identity(source.h))==argv[5],L"压缩包已改变");BY_HANDLE_FILE_INFORMATION meta{};require(GetFileInformationByHandle(source.h,&meta)&&((((uint64_t)meta.nFileSizeHigh<<32)|meta.nFileSizeLow)==_wcstoui64(argv[8],nullptr,10))&&((((uint64_t)meta.ftLastWriteTime.dwHighDateTime<<32)|meta.ftLastWriteTime.dwLowDateTime)==_wcstoui64(argv[7],nullptr,10)),L"压缩包内容已改变");auto input=new InStream(std::move(source));IInArchive*arc=nullptr;OpenCallback*open=new OpenCallback();
      for(unsigned char type:{(unsigned char)1,(unsigned char)7,(unsigned char)3,(unsigned char)0xCC}){auto id=format(type);IInArchive*test=nullptr;if(FAILED(factory(&id,&IID_IInArchive,(void**)&test)))continue;input->Seek(0,0,nullptr);UInt64 start=0;auto result=test->Open(input,&start,open);if(result==S_OK){arc=test;break;}test->Release();}
      open->Release();require(arc!=nullptr,L"压缩包损坏、需要密码或格式不支持");UInt32 count=0;require(SUCCEEDED(arc->GetNumberOfItems(&count))&&count<=MaxItems,L"压缩包项目超过一百万上限");totalItems=count;std::set<std::wstring>names;uint64_t units=0;
      for(UInt32 i=0;i<count;i++){validate(arc,i);auto p=itemPath(arc,i);units+=p.size();require(units<64ULL*1024*1024&&p.size()+wcslen(argv[3])<32760,L"压缩包路径超过资源上限");require(names.insert(normalized(p)).second,L"压缩包中有 Windows 同名冲突");auto n=sizeProp(arc,i);require(n<=MaxBytes-totalBytes,L"展开大小超过 1 TiB 上限");totalBytes+=n;}
      plan();for(UInt32 i=0;i<count;i++){std::cout<<"{\"type\":\"item\",\"index\":"<<i<<",\"path\":"<<json(itemPath(arc,i),true)<<",\"directory\":"<<(boolProp(arc,i,kpidIsDir)?"true":"false")<<",\"size\":"<<sizeProp(arc,i)<<"}\n"<<std::flush;}OutputRoot output(argv[3],argv[6]);auto callback=new ExtractCallback(arc,output);auto result=arc->Extract(nullptr,(UInt32)-1,0,callback);callback->Release();arc->Close();arc->Release();input->Release();require(result==S_OK,L"提取未完成");
    }
    finished=totalItems;progress(L"",true);std::cout<<"{\"type\":\"complete\"}\n"<<std::flush;return 0;
  }catch(...){try{std::cout<<"{\"type\":\"error\",\"message\":"<<json(failure.empty()?L"归档失败":failure)<<"}\n"<<std::flush;}catch(...){}return 1;}
}
